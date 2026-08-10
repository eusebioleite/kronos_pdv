create or replace trigger kronos.kronos_pdv_pedvenda after
	update or delete on kronos.f_pedvenda
	for each row
begin
	if
		deleting
		and :old.pdv_tipped = 'F'
	then
		update inventario.cards
		   set
			status = 'EXCLUIR'
		 where tipo = 'FECHADO'
		   and pedido = :old.pdv_numped
		   and indice = 0;
		return;
	end if;

	if
		updating
		and (
			:new.pdv_tipped = 'F'
			and :old.pdv_tipped = 'F'
		)
		and (
			:new.pdv_status = 'A'
			and :old.pdv_status <> 'A'
		)
	then
		insert into inventario.cards (
			tipo,pedido,status
		) values ( 'FECHADO',:new.pdv_numped,'NOVO' );
	end if;
exception
	when others then
		null;
end;
/